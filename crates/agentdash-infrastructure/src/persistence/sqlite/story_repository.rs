use sqlx::SqlitePool;

use agentdash_domain::common::error::DomainError;
use agentdash_domain::story::{
    ChangeKind, StateChange, Story, StoryContext, StoryRepository, StoryStatus,
};

pub struct SqliteStoryRepository {
    pool: SqlitePool,
}

impl SqliteStoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS stories (
                id TEXT PRIMARY KEY,
                project_id TEXT REFERENCES projects(id),
                backend_id TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'created',
                context TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_stories_project ON stories(project_id);

            CREATE TABLE IF NOT EXISTS state_changes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL DEFAULT '{}',
                backend_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_state_changes_entity ON state_changes(entity_id);
            CREATE INDEX IF NOT EXISTS idx_state_changes_backend ON state_changes(backend_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn record_change(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        entity_id: uuid::Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: &str,
    ) -> Result<(), DomainError> {
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
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl StoryRepository for SqliteStoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "INSERT INTO stories (id, project_id, backend_id, title, description, status, context, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(story.id.to_string())
        .bind(story.project_id.to_string())
        .bind(&story.backend_id)
        .bind(&story.title)
        .bind(&story.description)
        .bind(serde_json::to_string(&story.status)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.context)?)
        .bind(story.created_at.to_rfc3339())
        .bind(story.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.record_change(
            &mut tx,
            story.id,
            ChangeKind::StoryCreated,
            serde_json::to_value(story).unwrap_or_default(),
            &story.backend_id,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Story>, DomainError> {
        let row = sqlx::query_as::<_, StoryRow>(
            "SELECT id, project_id, backend_id, title, description, status, context, created_at, updated_at
             FROM stories WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_by_backend(&self, backend_id: &str) -> Result<Vec<Story>, DomainError> {
        let rows = sqlx::query_as::<_, StoryRow>(
            "SELECT id, project_id, backend_id, title, description, status, context, created_at, updated_at
             FROM stories WHERE backend_id = ? ORDER BY created_at DESC",
        )
        .bind(backend_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn list_by_project(&self, project_id: uuid::Uuid) -> Result<Vec<Story>, DomainError> {
        let rows = sqlx::query_as::<_, StoryRow>(
            "SELECT id, project_id, backend_id, title, description, status, context, created_at, updated_at
             FROM stories WHERE project_id = ? ORDER BY created_at DESC",
        )
        .bind(project_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn update(&self, story: &Story) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let result = sqlx::query(
            "UPDATE stories SET project_id = ?, backend_id = ?, title = ?, description = ?, status = ?, context = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(story.project_id.to_string())
        .bind(&story.backend_id)
        .bind(&story.title)
        .bind(&story.description)
        .bind(serde_json::to_string(&story.status)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.context)?)
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(story.id.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "story",
                id: story.id.to_string(),
            });
        }

        self.record_change(
            &mut tx,
            story.id,
            ChangeKind::StoryUpdated,
            serde_json::to_value(story).unwrap_or_default(),
            &story.backend_id,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM stories WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(DomainError::NotFound {
                entity: "story",
                id: id.to_string(),
            });
        }
        Ok(())
    }

    async fn get_changes_since(
        &self,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, DomainError> {
        let rows = sqlx::query_as::<_, StateChangeRow>(
            "SELECT id, entity_id, kind, payload, backend_id, created_at
             FROM state_changes WHERE id > ? ORDER BY id ASC LIMIT ?",
        )
        .bind(since_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn latest_event_id(&self) -> Result<i64, DomainError> {
        let row: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(id), 0) FROM state_changes")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(row.0)
    }

    async fn append_change(
        &self,
        entity_id: uuid::Uuid,
        kind: ChangeKind,
        payload: serde_json::Value,
        backend_id: &str,
    ) -> Result<(), DomainError> {
        sqlx::query(
            "INSERT INTO state_changes (entity_id, kind, payload, backend_id, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(entity_id.to_string())
        .bind(serde_json::to_string(&kind)?.trim_matches('"'))
        .bind(payload.to_string())
        .bind(backend_id)
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }
}

// --- SQLx 行映射辅助结构 ---

#[derive(sqlx::FromRow)]
struct StoryRow {
    id: String,
    project_id: Option<String>,
    backend_id: String,
    title: String,
    description: String,
    status: String,
    context: String,
    created_at: String,
    updated_at: String,
}

impl TryFrom<StoryRow> for Story {
    type Error = DomainError;

    fn try_from(row: StoryRow) -> Result<Self, Self::Error> {
        let project_id = row
            .project_id
            .as_deref()
            .unwrap_or("00000000-0000-0000-0000-000000000000")
            .parse()
            .unwrap_or_default();

        let context: StoryContext = serde_json::from_str(&row.context).unwrap_or_default();

        Ok(Story {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "story",
                id: row.id.clone(),
            })?,
            project_id,
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
            context,
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
    type Error = DomainError;

    fn try_from(row: StateChangeRow) -> Result<Self, Self::Error> {
        Ok(StateChange {
            id: row.id,
            entity_id: row.entity_id.parse().map_err(|_| DomainError::NotFound {
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
