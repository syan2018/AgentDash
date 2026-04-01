use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::story::ChangeKind;
use agentdash_domain::task::{
    AgentBinding, Artifact, Task, TaskExecutionMode, TaskRepository, TaskStatus,
};

pub struct SqliteTaskRepository {
    pool: PgPool,
}

impl SqliteTaskRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                story_id TEXT NOT NULL REFERENCES stories(id),
                workspace_id TEXT REFERENCES workspaces(id),
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                session_id TEXT,
                executor_session_id TEXT,
                execution_mode TEXT NOT NULL DEFAULT 'standard',
                agent_binding TEXT NOT NULL DEFAULT '{}',
                artifacts TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_story_id ON tasks(story_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_workspace_id ON tasks(workspace_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_session_id ON tasks(session_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_executor_session_id ON tasks(executor_session_id);
            CREATE INDEX IF NOT EXISTS idx_tasks_project_id ON tasks(project_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn load_story_snapshot(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        story_id: uuid::Uuid,
    ) -> Result<StorySnapshotRow, DomainError> {
        let story = sqlx::query_as::<_, StorySnapshotRow>(
            "SELECT id, project_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at
             FROM stories WHERE id = $1",
        )
        .bind(story_id.to_string())
        .fetch_optional(&mut **tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "story",
            id: story_id.to_string(),
        })?;

        Ok(story)
    }

    async fn insert_state_change(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        project_id: uuid::Uuid,
        entity_id: uuid::Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: Option<&str>,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO state_changes (project_id, entity_id, kind, payload, backend_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(project_id.to_string())
        .bind(entity_id.to_string())
        .bind(serde_json::to_string(&kind)?.trim_matches('"'))
        .bind(payload.to_string())
        .bind(backend_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut **tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl TaskRepository for SqliteTaskRepository {
    async fn create(&self, task: &Task) -> Result<(), DomainError> {
        self.create_task_with_story_update(task).await
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Task>, DomainError> {
        let row = sqlx::query_as::<_, TaskRow>(
            "SELECT id, project_id, story_id, workspace_id, title, description, status, session_id, executor_session_id, execution_mode, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_by_project(&self, project_id: uuid::Uuid) -> Result<Vec<Task>, DomainError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, project_id, story_id, workspace_id, title, description, status, session_id, executor_session_id, execution_mode, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE project_id = $1 ORDER BY created_at ASC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn list_by_story(&self, story_id: uuid::Uuid) -> Result<Vec<Task>, DomainError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, project_id, story_id, workspace_id, title, description, status, session_id, executor_session_id, execution_mode, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE story_id = $1 ORDER BY created_at ASC",
        )
        .bind(story_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn list_by_workspace(&self, workspace_id: uuid::Uuid) -> Result<Vec<Task>, DomainError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, project_id, story_id, workspace_id, title, description, status, session_id, executor_session_id, execution_mode, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE workspace_id = $1 ORDER BY created_at ASC",
        )
        .bind(workspace_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn update(&self, task: &Task) -> Result<(), DomainError> {
        let result = sqlx::query(
            "UPDATE tasks SET project_id = $1, story_id = $2, workspace_id = $3, title = $4, description = $5, status = $6, session_id = $7, executor_session_id = $8, execution_mode = $9, agent_binding = $10, artifacts = $11, updated_at = $12
             WHERE id = $13",
        )
        .bind(task.project_id.to_string())
        .bind(task.story_id.to_string())
        .bind(task.workspace_id.map(|id| id.to_string()))
        .bind(&task.title)
        .bind(&task.description)
        .bind(serde_json::to_string(&task.status)?.trim_matches('"'))
        .bind(task.session_id.as_deref())
        .bind(task.executor_session_id.as_deref())
        .bind(serde_json::to_string(&task.execution_mode)?.trim_matches('"'))
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
        let result = sqlx::query("UPDATE tasks SET status = $1, updated_at = $2 WHERE id = $3")
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
        self.delete_task_with_story_update(id).await?;
        Ok(())
    }

    async fn create_task_with_story_update(&self, task: &Task) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let mut story = self.load_story_snapshot(&mut tx, task.story_id).await?;
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO tasks (id, project_id, story_id, workspace_id, title, description, status, session_id, executor_session_id, execution_mode, agent_binding, artifacts, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(task.id.to_string())
        .bind(task.project_id.to_string())
        .bind(task.story_id.to_string())
        .bind(task.workspace_id.map(|id| id.to_string()))
        .bind(&task.title)
        .bind(&task.description)
        .bind(serde_json::to_string(&task.status)?.trim_matches('"'))
        .bind(task.session_id.as_deref())
        .bind(task.executor_session_id.as_deref())
        .bind(serde_json::to_string(&task.execution_mode)?.trim_matches('"'))
        .bind(serde_json::to_string(&task.agent_binding)?)
        .bind(serde_json::to_string(&task.artifacts)?)
        .bind(task.created_at.to_rfc3339())
        .bind(task.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE stories SET task_count = task_count + 1, updated_at = $1 WHERE id = $2",
        )
        .bind(&now)
        .bind(task.story_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "story",
                id: task.story_id.to_string(),
            });
        }

        story.task_count += 1;
        story.updated_at = now;

        self.insert_state_change(
            &mut tx,
            task.project_id,
            task.id,
            ChangeKind::TaskCreated,
            build_task_created_payload(task),
            None,
        )
        .await?;
        self.insert_state_change(
            &mut tx,
            task.project_id,
            task.story_id,
            ChangeKind::StoryUpdated,
            story.to_payload("task_created_by_user"),
            None,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete_task_with_story_update(
        &self,
        task_id: uuid::Uuid,
    ) -> Result<Task, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let row = sqlx::query_as::<_, TaskRow>(
            "SELECT id, project_id, story_id, workspace_id, title, description, status, session_id, executor_session_id, execution_mode, agent_binding, artifacts, created_at, updated_at
             FROM tasks WHERE id = $1",
        )
        .bind(task_id.to_string())
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?
        .ok_or_else(|| DomainError::NotFound {
            entity: "task",
            id: task_id.to_string(),
        })?;
        let task: Task = row.try_into()?;

        let mut story = self.load_story_snapshot(&mut tx, task.story_id).await?;
        let now = chrono::Utc::now().to_rfc3339();

        let deleted = sqlx::query("DELETE FROM tasks WHERE id = $1")
            .bind(task_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        if deleted.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "task",
                id: task_id.to_string(),
            });
        }

        let result = sqlx::query(
            "UPDATE stories
             SET task_count = MAX(task_count - 1, 0), updated_at = $1
             WHERE id = $2",
        )
        .bind(&now)
        .bind(task.story_id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "story",
                id: task.story_id.to_string(),
            });
        }

        story.task_count = (story.task_count - 1).max(0);
        story.updated_at = now;

        self.insert_state_change(
            &mut tx,
            task.project_id,
            task_id,
            ChangeKind::TaskDeleted,
            serde_json::json!({
                "task_id": task_id,
                "project_id": task.project_id,
                "story_id": task.story_id,
                "reason": "task_deleted_by_user"
            }),
            None,
        )
        .await?;
        self.insert_state_change(
            &mut tx,
            task.project_id,
            task.story_id,
            ChangeKind::StoryUpdated,
            story.to_payload("task_deleted_by_user"),
            None,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(task)
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct TaskRow {
    id: String,
    project_id: String,
    story_id: String,
    workspace_id: Option<String>,
    title: String,
    description: String,
    status: String,
    session_id: Option<String>,
    executor_session_id: Option<String>,
    execution_mode: String,
    agent_binding: String,
    artifacts: String,
    created_at: String,
    updated_at: String,
}

#[derive(sqlx::FromRow)]
struct StorySnapshotRow {
    id: String,
    project_id: String,
    title: String,
    description: String,
    status: String,
    priority: String,
    story_type: String,
    tags: String,
    task_count: i64,
    context: String,
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
            project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.project_id.clone(),
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
            executor_session_id: row.executor_session_id,
            execution_mode: match row.execution_mode.as_str() {
                "auto_retry" => TaskExecutionMode::AutoRetry,
                "one_shot" => TaskExecutionMode::OneShot,
                _ => TaskExecutionMode::Standard,
            },
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

fn build_task_created_payload(task: &Task) -> serde_json::Value {
    let mut payload = serde_json::to_value(task).unwrap_or_default();
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "reason".to_string(),
            serde_json::Value::String("task_created_by_user".to_string()),
        );
        return payload;
    }

    serde_json::json!({
        "task_id": task.id,
        "project_id": task.project_id,
        "story_id": task.story_id,
        "reason": "task_created_by_user"
    })
}

impl StorySnapshotRow {
    fn to_payload(&self, reason: &str) -> serde_json::Value {
        let tags: serde_json::Value =
            serde_json::from_str(&self.tags).unwrap_or_else(|_| serde_json::json!([]));
        let context: serde_json::Value =
            serde_json::from_str(&self.context).unwrap_or_else(|_| serde_json::json!({}));

        serde_json::json!({
            "id": self.id.clone(),
            "project_id": self.project_id.clone(),
            "title": self.title.clone(),
            "description": self.description.clone(),
            "status": self.status.clone(),
            "priority": self.priority.clone(),
            "story_type": self.story_type.clone(),
            "tags": tags,
            "task_count": self.task_count,
            "context": context,
            "created_at": self.created_at.clone(),
            "updated_at": self.updated_at.clone(),
            "reason": reason
        })
    }
}
