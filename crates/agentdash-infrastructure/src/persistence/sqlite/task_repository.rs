use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::task::{Task, TaskStatus, TaskRepository};

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
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                agent_type TEXT,
                agent_pid TEXT,
                workspace_path TEXT,
                artifacts TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_story_id ON tasks(story_id);
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
    async fn list_by_story(&self, story_id: uuid::Uuid) -> Result<Vec<Task>, DomainError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, story_id, title, description, status, agent_type, agent_pid, workspace_path, artifacts, created_at, updated_at
             FROM tasks WHERE story_id = ? ORDER BY created_at ASC",
        )
        .bind(story_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct TaskRow {
    id: String,
    story_id: String,
    title: String,
    description: String,
    status: String,
    agent_type: Option<String>,
    agent_pid: Option<String>,
    workspace_path: Option<String>,
    artifacts: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<TaskRow> for Task {
    type Error = DomainError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        Ok(Task {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "task",
                id: row.id.clone(),
            })?,
            story_id: row.story_id.parse().map_err(|_| DomainError::NotFound {
                entity: "story",
                id: row.story_id.clone(),
            })?,
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
            agent_type: row.agent_type,
            agent_pid: row.agent_pid,
            workspace_path: row.workspace_path,
            artifacts: serde_json::from_str(&row.artifacts).unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}
