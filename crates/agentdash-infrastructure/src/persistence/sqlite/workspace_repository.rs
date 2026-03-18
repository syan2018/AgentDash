use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::workspace::{
    GitConfig, Workspace, WorkspaceRepository, WorkspaceStatus, WorkspaceType,
};

pub struct SqliteWorkspaceRepository {
    pool: SqlitePool,
}

impl SqliteWorkspaceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                backend_id TEXT NOT NULL,
                name TEXT NOT NULL,
                container_ref TEXT NOT NULL,
                workspace_type TEXT NOT NULL DEFAULT 'git_worktree',
                status TEXT NOT NULL DEFAULT 'pending',
                git_config TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_workspaces_project ON workspaces(project_id);
            CREATE INDEX IF NOT EXISTS idx_workspaces_status ON workspaces(status);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let has_backend_id = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pragma_table_info('workspaces') WHERE name = 'backend_id'",
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if has_backend_id == 0 {
            sqlx::query("ALTER TABLE workspaces ADD COLUMN backend_id TEXT NOT NULL DEFAULT ''")
                .execute(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl WorkspaceRepository for SqliteWorkspaceRepository {
    async fn create(&self, workspace: &Workspace) -> Result<(), DomainError> {
        let git_config_json = workspace
            .git_config
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        sqlx::query(
            "INSERT INTO workspaces (id, project_id, backend_id, name, container_ref, workspace_type, status, git_config, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(workspace.id.to_string())
        .bind(workspace.project_id.to_string())
        .bind(&workspace.backend_id)
        .bind(&workspace.name)
        .bind(&workspace.container_ref)
        .bind(workspace_type_to_str(&workspace.workspace_type))
        .bind(workspace_status_to_str(&workspace.status))
        .bind(git_config_json)
        .bind(workspace.created_at.to_rfc3339())
        .bind(workspace.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Workspace>, DomainError> {
        let row = sqlx::query_as::<_, WorkspaceRow>(
            "SELECT id, project_id, backend_id, name, container_ref, workspace_type, status, git_config, created_at, updated_at
             FROM workspaces WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_by_project(&self, project_id: uuid::Uuid) -> Result<Vec<Workspace>, DomainError> {
        let rows = sqlx::query_as::<_, WorkspaceRow>(
            "SELECT id, project_id, backend_id, name, container_ref, workspace_type, status, git_config, created_at, updated_at
             FROM workspaces WHERE project_id = ? ORDER BY created_at DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn update(&self, workspace: &Workspace) -> Result<(), DomainError> {
        let git_config_json = workspace
            .git_config
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        let result = sqlx::query(
            "UPDATE workspaces SET backend_id = ?, name = ?, container_ref = ?, workspace_type = ?, status = ?, git_config = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&workspace.backend_id)
        .bind(&workspace.name)
        .bind(&workspace.container_ref)
        .bind(workspace_type_to_str(&workspace.workspace_type))
        .bind(workspace_status_to_str(&workspace.status))
        .bind(git_config_json)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(workspace.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "workspace",
                id: workspace.id.to_string(),
            });
        }
        Ok(())
    }

    async fn update_status(
        &self,
        id: uuid::Uuid,
        status: WorkspaceStatus,
    ) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE workspaces SET status = ?, updated_at = ? WHERE id = ?")
            .bind(workspace_status_to_str(&status))
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "workspace",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM workspaces WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "workspace",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

// --- 辅助函数 ---

fn workspace_type_to_str(wt: &WorkspaceType) -> &'static str {
    match wt {
        WorkspaceType::GitWorktree => "git_worktree",
        WorkspaceType::Static => "static",
        WorkspaceType::Ephemeral => "ephemeral",
    }
}

fn workspace_status_to_str(ws: &WorkspaceStatus) -> &'static str {
    match ws {
        WorkspaceStatus::Pending => "pending",
        WorkspaceStatus::Preparing => "preparing",
        WorkspaceStatus::Ready => "ready",
        WorkspaceStatus::Active => "active",
        WorkspaceStatus::Archived => "archived",
        WorkspaceStatus::Error => "error",
    }
}

fn str_to_workspace_type(s: &str) -> WorkspaceType {
    match s {
        "git_worktree" => WorkspaceType::GitWorktree,
        "static" => WorkspaceType::Static,
        "ephemeral" => WorkspaceType::Ephemeral,
        _ => WorkspaceType::GitWorktree,
    }
}

fn str_to_workspace_status(s: &str) -> WorkspaceStatus {
    match s {
        "pending" => WorkspaceStatus::Pending,
        "preparing" => WorkspaceStatus::Preparing,
        "ready" => WorkspaceStatus::Ready,
        "active" => WorkspaceStatus::Active,
        "archived" => WorkspaceStatus::Archived,
        "error" => WorkspaceStatus::Error,
        _ => WorkspaceStatus::Pending,
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct WorkspaceRow {
    id: String,
    project_id: String,
    backend_id: String,
    name: String,
    container_ref: String,
    workspace_type: String,
    status: String,
    git_config: Option<String>,
    created_at: String,
    updated_at: String,
}

impl TryFrom<WorkspaceRow> for Workspace {
    type Error = DomainError;

    fn try_from(row: WorkspaceRow) -> Result<Self, Self::Error> {
        let git_config = row
            .git_config
            .as_deref()
            .map(serde_json::from_str::<GitConfig>)
            .transpose()
            .ok()
            .flatten();

        Ok(Workspace {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "workspace",
                id: row.id.clone(),
            })?,
            project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.project_id.clone(),
            })?,
            backend_id: row.backend_id,
            name: row.name,
            container_ref: row.container_ref,
            workspace_type: str_to_workspace_type(&row.workspace_type),
            status: str_to_workspace_status(&row.status),
            git_config,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}
