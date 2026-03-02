use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::task::{AgentBinding, Artifact, Task, TaskRepository, TaskStatus};

pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

impl SqliteTaskRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                story_id TEXT NOT NULL REFERENCES stories(id),
                workspace_id TEXT REFERENCES workspaces(id),
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                session_id TEXT,
                agent_binding TEXT NOT NULL DEFAULT '{}',
                artifacts TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_story_id ON tasks(story_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_workspace_id ON tasks(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_session_id ON tasks(session_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl TaskRepository for SqliteTaskRepository {
    async fn create(&self, task: &Task) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO tasks (id, story_id, workspace_id, title, description, status, session_id, agent_binding, artifacts, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(task.id.to_string())
        .bind(task.story_id.to_string())
        .bind(task.workspace_id.map(|id| id.to_string()))
        .bind(&task.title)
        .bind(&task.description)
        .bind(serde_json::to_string(&task.status)?.trim_matches('"'))
        .bind(task.session_id.as_deref())
        .bind(serde_json::to_string(&task.agent_binding)?)
        .bind(serde_json::to_string(&task.artifacts)?)
        .bind(task.created_at.to_rfc3339())
        .bind(task.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Task>, DomainError> {
        let row = sqlx::query_as::<_, TaskRow>(
            "SELECT id, story_id, workspace_id, title, description, status, session_id, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_by_story(&self, story_id: uuid::Uuid) -> Result<Vec<Task>, DomainError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, story_id, workspace_id, title, description, status, session_id, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE story_id = ? ORDER BY created_at ASC",
        )
        .bind(story_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn list_by_workspace(&self, workspace_id: uuid::Uuid) -> Result<Vec<Task>, DomainError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, story_id, workspace_id, title, description, status, session_id, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE workspace_id = ? ORDER BY created_at ASC",
        )
        .bind(workspace_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn update(&self, task: &Task) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE tasks SET story_id = ?, workspace_id = ?, title = ?, description = ?, status = ?, session_id = ?, agent_binding = ?, artifacts = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(task.story_id.to_string())
        .bind(task.workspace_id.map(|id| id.to_string()))
        .bind(&task.title)
        .bind(&task.description)
        .bind(serde_json::to_string(&task.status)?.trim_matches('"'))
        .bind(task.session_id.as_deref())
        .bind(serde_json::to_string(&task.agent_binding)?)
        .bind(serde_json::to_string(&task.artifacts)?)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(task.id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "task",
                id: task.id.to_string(),
            });
        }
        Ok(())
    }

    async fn update_status(&self, id: uuid::Uuid, status: TaskStatus) -> Result<(), DomainError> {
        let result = sqlx::query("UPDATE tasks SET status = ?, updated_at = ? WHERE id = ?")
            .bind(serde_json::to_string(&status)?.trim_matches('"'))
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "task",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "task",
                id: id.to_string(),
            });
        }
        Ok(())
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct TaskRow {
    id: String,
    story_id: String,
    workspace_id: Option<String>,
    title: String,
    description: String,
    status: String,
    session_id: Option<String>,
    agent_binding: String,
    artifacts: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<TaskRow> for Task {
    type Error = DomainError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        let workspace_id = row.workspace_id.as_deref().and_then(|s| s.parse().ok());

        let agent_binding: AgentBinding =
            serde_json::from_str(&row.agent_binding).unwrap_or_default();

        let artifacts: Vec<Artifact> = serde_json::from_str(&row.artifacts).unwrap_or_default();

        Ok(Task {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "task",
                id: row.id.clone(),
            })?,
            story_id: row.story_id.parse().map_err(|_| DomainError::NotFound {
                entity: "story",
                id: row.story_id.clone(),
            })?,
            workspace_id,
            title: row.title,
            description: row.description,
            status: match row.status.as_str() {
                "pending" => TaskStatus::Pending,
                "assigned" => TaskStatus::Assigned,
                "running" => TaskStatus::Running,
                "awaiting_verification" => TaskStatus::AwaitingVerification,
                "completed" => TaskStatus::Completed,
                "failed" => TaskStatus::Failed,
                _ => TaskStatus::Pending,
            },
            session_id: row.session_id,
            agent_binding,
            artifacts,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}
