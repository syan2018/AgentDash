use sqlx::SqlitePool;

use crate::error::StateError;
use crate::models::{Story, StoryStatus, Task, TaskStatus, StateChange};
use crate::models::state_change::ChangeKind;

/// StateStore — 状态存储的核心入口
///
/// 封装所有对 SQLite 的读写操作，保证：
/// 1. 状态变更原子性
/// 2. StateChange 日志的完整写入
/// 3. 并发安全
pub struct StateStore {
    pool: SqlitePool,
}

impl StateStore {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// 初始化数据库表结构
    pub async fn initialize(&self) -> Result<(), StateError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS stories (
                id TEXT PRIMARY KEY,
                backend_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'created',
                context TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

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

            CREATE TABLE IF NOT EXISTS state_changes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL DEFAULT '{}',
                backend_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_tasks_story_id ON tasks(story_id);
            CREATE INDEX IF NOT EXISTS idx_state_changes_entity ON state_changes(entity_id);
            CREATE INDEX IF NOT EXISTS idx_state_changes_backend ON state_changes(backend_id);
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 创建 Story
    pub async fn create_story(&self, story: &Story) -> Result<(), StateError> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO stories (id, backend_id, title, description, status, context, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(story.id.to_string())
        .bind(&story.backend_id)
        .bind(&story.title)
        .bind(&story.description)
        .bind(serde_json::to_string(&story.status)?.trim_matches('"'))
        .bind(story.context.to_string())
        .bind(story.created_at.to_rfc3339())
        .bind(story.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await?;

        self.record_change(
            &mut tx,
            story.id,
            ChangeKind::StoryCreated,
            serde_json::to_value(story).unwrap_or_default(),
            &story.backend_id,
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// 获取指定 Backend 的所有 Story
    pub async fn list_stories(&self, backend_id: &str) -> Result<Vec<Story>, StateError> {
        let rows = sqlx::query_as::<_, StoryRow>(
            "SELECT id, backend_id, title, description, status, context, created_at, updated_at
             FROM stories WHERE backend_id = ? ORDER BY created_at DESC",
        )
        .bind(backend_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// 获取 Story 下的所有 Task
    pub async fn list_tasks(&self, story_id: uuid::Uuid) -> Result<Vec<Task>, StateError> {
        let rows = sqlx::query_as::<_, TaskRow>(
            "SELECT id, story_id, title, description, status, agent_type, agent_pid, workspace_path, artifacts, created_at, updated_at
             FROM tasks WHERE story_id = ? ORDER BY created_at ASC",
        )
        .bind(story_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// 获取 since_id 之后的所有状态变更（Resume 核心接口）
    pub async fn get_changes_since(
        &self,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, StateError> {
        let rows = sqlx::query_as::<_, StateChangeRow>(
            "SELECT id, entity_id, kind, payload, backend_id, created_at
             FROM state_changes WHERE id > ? ORDER BY id ASC LIMIT ?",
        )
        .bind(since_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    /// 获取当前最新的 event_id
    pub async fn latest_event_id(&self) -> Result<i64, StateError> {
        let row: (i64,) =
            sqlx::query_as("SELECT COALESCE(MAX(id), 0) FROM state_changes")
                .fetch_one(&self.pool)
                .await?;
        Ok(row.0)
    }

    async fn record_change(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        entity_id: uuid::Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: &str,
    ) -> Result<(), StateError> {
        sqlx::query(
            "INSERT INTO state_changes (entity_id, kind, payload, backend_id, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(entity_id.to_string())
        .bind(serde_json::to_string(&kind)?.trim_matches('"'))
        .bind(payload.to_string())
        .bind(backend_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&mut **tx)
        .await?;

        Ok(())
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct StoryRow {
    id: String,
    backend_id: String,
    title: String,
    description: String,
    status: String,
    context: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<StoryRow> for Story {
    type Error = StateError;

    fn try_from(row: StoryRow) -> Result<Self, Self::Error> {
        Ok(Story {
            id: row.id.parse().map_err(|_| StateError::NotFound {
                entity: "story",
                id: row.id.clone(),
            })?,
            backend_id: row.backend_id,
            title: row.title,
            description: row.description,
            status: match row.status.as_str() {
                "created" => StoryStatus::Created,
                "context_ready" => StoryStatus::ContextReady,
                "decomposed" => StoryStatus::Decomposed,
                "executing" => StoryStatus::Executing,
                "completed" => StoryStatus::Completed,
                "failed" => StoryStatus::Failed,
                _ => StoryStatus::Created,
            },
            context: serde_json::from_str(&row.context).unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&row.updated_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}

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
    type Error = StateError;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        Ok(Task {
            id: row.id.parse().map_err(|_| StateError::NotFound {
                entity: "task",
                id: row.id.clone(),
            })?,
            story_id: row.story_id.parse().map_err(|_| StateError::NotFound {
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

#[derive(sqlx::FromRow)]
struct StateChangeRow {
    id: i64,
    entity_id: String,
    kind: String,
    payload: String,
    backend_id: String,
    created_at: String,
}

impl TryFrom<StateChangeRow> for StateChange {
    type Error = StateError;

    fn try_from(row: StateChangeRow) -> Result<Self, Self::Error> {
        Ok(StateChange {
            id: row.id,
            entity_id: row.entity_id.parse().map_err(|_| StateError::NotFound {
                entity: "state_change",
                id: row.entity_id.clone(),
            })?,
            kind: match row.kind.as_str() {
                "story_created" => ChangeKind::StoryCreated,
                "story_updated" => ChangeKind::StoryUpdated,
                "story_status_changed" => ChangeKind::StoryStatusChanged,
                "task_created" => ChangeKind::TaskCreated,
                "task_updated" => ChangeKind::TaskUpdated,
                "task_status_changed" => ChangeKind::TaskStatusChanged,
                "task_artifact_added" => ChangeKind::TaskArtifactAdded,
                _ => ChangeKind::StoryUpdated,
            },
            payload: serde_json::from_str(&row.payload).unwrap_or_default(),
            backend_id: row.backend_id,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        })
    }
}
