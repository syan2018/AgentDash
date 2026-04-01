use sqlx::PgPool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::story::ChangeKind;
use agentdash_domain::task::{
    AgentBinding, Artifact, Task, TaskAggregateCommandRepository, TaskExecutionMode,
    TaskRepository, TaskStatus,
};

use super::state_change_store::{append_state_change_in_tx, initialize_state_changes_schema};

pub struct PostgresTaskRepository {
    pool: PgPool,
}

impl PostgresTaskRepository {
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

        initialize_state_changes_schema(&self.pool).await?;

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
}

#[async_trait::async_trait]
impl TaskRepository for PostgresTaskRepository {
    async fn create(&self, task: &Task) -> Result<(), DomainError> {
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
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
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
        let result = sqlx::query("DELETE FROM tasks WHERE id = $1")
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

#[async_trait::async_trait]
impl TaskAggregateCommandRepository for PostgresTaskRepository {
    async fn create_for_story(&self, task: &Task) -> Result<(), DomainError> {
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

        append_state_change_in_tx(
            &mut tx,
            task.project_id,
            task.id,
            ChangeKind::TaskCreated,
            build_task_created_payload(task),
            None,
        )
        .await?;
        append_state_change_in_tx(
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

    async fn delete_for_story(&self, task_id: uuid::Uuid) -> Result<Task, DomainError> {
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

        append_state_change_in_tx(
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
        append_state_change_in_tx(
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
        let workspace_id = row
            .workspace_id
            .as_deref()
            .map(|value| {
                value.parse().map_err(|error| {
                    DomainError::InvalidConfig(format!("tasks.workspace_id: {error}"))
                })
            })
            .transpose()?;

        let agent_binding: AgentBinding =
            parse_json_column(&row.agent_binding, "tasks.agent_binding")?;

        let artifacts: Vec<Artifact> = parse_json_column(&row.artifacts, "tasks.artifacts")?;

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
            status: parse_task_status(&row.status)?,
            session_id: row.session_id,
            executor_session_id: row.executor_session_id,
            execution_mode: parse_task_execution_mode(&row.execution_mode)?,
            agent_binding,
            artifacts,
            created_at: super::parse_pg_timestamp(&row.created_at),
            updated_at: super::parse_pg_timestamp(&row.updated_at),
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

fn parse_json_column<T: serde::de::DeserializeOwned>(
    raw: &str,
    field: &str,
) -> Result<T, DomainError> {
    serde_json::from_str(raw)
        .map_err(|error| DomainError::InvalidConfig(format!("{field}: {error}")))
}

fn parse_task_status(raw: &str) -> Result<TaskStatus, DomainError> {
    match raw {
        "pending" => Ok(TaskStatus::Pending),
        "assigned" => Ok(TaskStatus::Assigned),
        "running" => Ok(TaskStatus::Running),
        "awaiting_verification" => Ok(TaskStatus::AwaitingVerification),
        "completed" => Ok(TaskStatus::Completed),
        "failed" => Ok(TaskStatus::Failed),
        _ => Err(DomainError::InvalidConfig(format!(
            "tasks.status: 未知状态 `{raw}`"
        ))),
    }
}

fn parse_task_execution_mode(raw: &str) -> Result<TaskExecutionMode, DomainError> {
    match raw {
        "standard" => Ok(TaskExecutionMode::Standard),
        "auto_retry" => Ok(TaskExecutionMode::AutoRetry),
        "one_shot" => Ok(TaskExecutionMode::OneShot),
        _ => Err(DomainError::InvalidConfig(format!(
            "tasks.execution_mode: 未知模式 `{raw}`"
        ))),
    }
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
