use sqlx::{PgPool, Row};

use agentdash_domain::common::error::DomainError;
use agentdash_domain::story::{
    ChangeKind, StateChange, Story, StoryContext, StoryPriority, StoryRepository, StoryStatus,
    StoryType,
};

pub struct SqliteStoryRepository {
    pool: PgPool,
}

impl SqliteStoryRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn initialize(&self) -> Result<(), DomainError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS stories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                default_workspace_id TEXT,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'created',
                priority TEXT NOT NULL DEFAULT 'p2',
                story_type TEXT NOT NULL DEFAULT 'feature',
                tags TEXT NOT NULL DEFAULT '[]',
                task_count INTEGER NOT NULL DEFAULT 0,
                context TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_stories_project ON stories(project_id);

            CREATE TABLE IF NOT EXISTS state_changes (
                id BIGSERIAL PRIMARY KEY,
                project_id TEXT NOT NULL DEFAULT '',
                entity_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                payload TEXT NOT NULL DEFAULT '{}',
                backend_id TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_state_changes_entity ON state_changes(entity_id);
            CREATE INDEX IF NOT EXISTS idx_state_changes_backend ON state_changes(backend_id);
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.ensure_story_column("default_workspace_id", "TEXT")
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_state_changes_project ON state_changes(project_id)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        Ok(())
    }

    async fn ensure_story_column(
        &self,
        column_name: &str,
        column_definition: &str,
    ) -> Result<(), DomainError> {
        let rows = sqlx::query(
            "SELECT column_name AS name
             FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'stories'",
        )
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        let column_exists = rows.iter().any(|row| {
            row.try_get::<String, _>("name")
                .map(|name| name == column_name)
                .unwrap_or(false)
        });

        if !column_exists {
            sqlx::query(&format!(
                "ALTER TABLE stories ADD COLUMN {column_name} {column_definition}"
            ))
            .execute(&self.pool)
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        }

        Ok(())
    }

    async fn record_change(
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
impl StoryRepository for SqliteStoryRepository {
    async fn create(&self, story: &Story) -> Result<(), DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        sqlx::query(
            "INSERT INTO stories (id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
        )
        .bind(story.id.to_string())
        .bind(story.project_id.to_string())
        .bind(story.default_workspace_id.map(|id| id.to_string()))
        .bind(&story.title)
        .bind(&story.description)
        .bind(serde_json::to_string(&story.status)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.priority)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.story_type)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.tags)?)
        .bind(story.task_count as i64)
        .bind(serde_json::to_string(&story.context)?)
        .bind(story.created_at.to_rfc3339())
        .bind(story.updated_at.to_rfc3339())
        .execute(&mut *tx)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        self.record_change(
            &mut tx,
            story.project_id,
            story.id,
            ChangeKind::StoryCreated,
            serde_json::to_value(story).unwrap_or_default(),
            None,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn get_by_id(&self, id: uuid::Uuid) -> Result<Option<Story>, DomainError> {
        let row = sqlx::query_as::<_, StoryRow>(
            "SELECT id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at
             FROM stories WHERE id = $1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        row.map(|r| r.try_into()).transpose()
    }

    async fn list_by_project(&self, project_id: uuid::Uuid) -> Result<Vec<Story>, DomainError> {
        let rows = sqlx::query_as::<_, StoryRow>(
            "SELECT id, project_id, default_workspace_id, title, description, status, priority, story_type, tags, task_count, context, created_at, updated_at
             FROM stories WHERE project_id = $1 ORDER BY created_at DESC",
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
            "UPDATE stories SET project_id = $1, default_workspace_id = $2, title = $3, description = $4, status = $5, priority = $6, story_type = $7, tags = $8, task_count = $9, context = $10, updated_at = $11
             WHERE id = $12",
        )
        .bind(story.project_id.to_string())
        .bind(story.default_workspace_id.map(|id| id.to_string()))
        .bind(&story.title)
        .bind(&story.description)
        .bind(serde_json::to_string(&story.status)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.priority)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.story_type)?.trim_matches('"'))
        .bind(serde_json::to_string(&story.tags)?)
        .bind(story.task_count as i64)
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
            story.project_id,
            story.id,
            ChangeKind::StoryUpdated,
            serde_json::to_value(story).unwrap_or_default(),
            None,
        )
        .await?;

        tx.commit()
            .await
            .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: uuid::Uuid) -> Result<(), DomainError> {
        let result = sqlx::query("DELETE FROM stories WHERE id = $1")
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
            "SELECT id, project_id, entity_id, kind, payload, backend_id, created_at
             FROM state_changes WHERE id > $1 ORDER BY id ASC LIMIT $2",
        )
        .bind(since_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;

        rows.into_iter().map(|r| r.try_into()).collect()
    }

    async fn get_changes_since_by_project(
        &self,
        project_id: uuid::Uuid,
        since_id: i64,
        limit: i64,
    ) -> Result<Vec<StateChange>, DomainError> {
        let rows = sqlx::query_as::<_, StateChangeRow>(
            "SELECT id, project_id, entity_id, kind, payload, backend_id, created_at
             FROM state_changes
             WHERE project_id = $1 AND id > $2
             ORDER BY id ASC
             LIMIT $3",
        )
        .bind(project_id.to_string())
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

    async fn latest_event_id_by_project(&self, project_id: uuid::Uuid) -> Result<i64, DomainError> {
        let row: (i64,) =
            sqlx::query_as("SELECT COALESCE(MAX(id), 0) FROM state_changes WHERE project_id = $1")
                .bind(project_id.to_string())
                .fetch_one(&self.pool)
                .await
                .map_err(|e| DomainError::InvalidConfig(e.to_string()))?;
        Ok(row.0)
    }

    async fn append_change(
        &self,
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
    project_id: String,
    default_workspace_id: Option<String>,
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

impl TryFrom<StoryRow> for Story {
    type Error = DomainError;

    fn try_from(row: StoryRow) -> Result<Self, Self::Error> {
        let project_id = row.project_id.parse().map_err(|_| DomainError::NotFound {
            entity: "story",
            id: row.id.clone(),
        })?;

        let context: StoryContext = serde_json::from_str(&row.context).unwrap_or_default();
        let tags: Vec<String> = serde_json::from_str(&row.tags).unwrap_or_default();

        let default_workspace_id = row
            .default_workspace_id
            .as_deref()
            .and_then(|s| s.parse().ok());

        Ok(Story {
            id: row.id.parse().map_err(|_| DomainError::NotFound {
                entity: "story",
                id: row.id.clone(),
            })?,
            project_id,
            default_workspace_id,
            title: row.title,
            description: row.description,
            status: match row.status.as_str() {
                "created" => StoryStatus::Created,
                "context_ready" => StoryStatus::ContextReady,
                "decomposed" => StoryStatus::Decomposed,
                "executing" => StoryStatus::Executing,
                "completed" => StoryStatus::Completed,
                "failed" => StoryStatus::Failed,
                "cancelled" => StoryStatus::Cancelled,
                "canceled" => StoryStatus::Cancelled,
                _ => StoryStatus::Created,
            },
            priority: match row.priority.as_str() {
                "p0" => StoryPriority::P0,
                "p1" => StoryPriority::P1,
                "p2" => StoryPriority::P2,
                "p3" => StoryPriority::P3,
                _ => StoryPriority::P2,
            },
            story_type: match row.story_type.as_str() {
                "feature" => StoryType::Feature,
                "bugfix" => StoryType::Bugfix,
                "refactor" => StoryType::Refactor,
                "docs" => StoryType::Docs,
                "test" => StoryType::Test,
                "other" => StoryType::Other,
                _ => StoryType::Feature,
            },
            tags,
            task_count: row.task_count.max(0) as u32,
            context,
            created_at: super::parse_pg_timestamp(&row.created_at),
            updated_at: super::parse_pg_timestamp(&row.updated_at),
        })
    }
}

#[derive(sqlx::FromRow)]
struct StateChangeRow {
    id: i64,
    project_id: String,
    entity_id: String,
    kind: String,
    payload: String,
    backend_id: Option<String>,
    created_at: String,
}

impl TryFrom<StateChangeRow> for StateChange {
    type Error = DomainError;

    fn try_from(row: StateChangeRow) -> Result<Self, Self::Error> {
        Ok(StateChange {
            id: row.id,
            project_id: row.project_id.parse().map_err(|_| DomainError::NotFound {
                entity: "project",
                id: row.project_id.clone(),
            })?,
            entity_id: row.entity_id.parse().map_err(|_| DomainError::NotFound {
                entity: "state_change",
                id: row.entity_id.clone(),
            })?,
            kind: match row.kind.as_str() {
                "story_created" => ChangeKind::StoryCreated,
                "story_updated" => ChangeKind::StoryUpdated,
                "story_status_changed" => ChangeKind::StoryStatusChanged,
                "story_deleted" => ChangeKind::StoryDeleted,
                "task_created" => ChangeKind::TaskCreated,
                "task_updated" => ChangeKind::TaskUpdated,
                "task_status_changed" => ChangeKind::TaskStatusChanged,
                "task_deleted" => ChangeKind::TaskDeleted,
                "task_artifact_added" => ChangeKind::TaskArtifactAdded,
                _ => ChangeKind::StoryUpdated,
            },
            payload: serde_json::from_str(&row.payload).unwrap_or_default(),
            backend_id: row.backend_id.unwrap_or_default(),
            created_at: super::parse_pg_timestamp(&row.created_at),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn new_repo_with_legacy_story_table() -> SqliteStoryRepository {
        let database_url =
            std::env::var("TEST_DATABASE_URL").expect("运行测试前需设置 TEST_DATABASE_URL");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("应能连接测试 PostgreSQL");

        sqlx::query(
            r#"
            CREATE TABLE projects (
                id TEXT PRIMARY KEY
            );

            CREATE TABLE stories (
                id TEXT PRIMARY KEY,
                project_id TEXT NOT NULL REFERENCES projects(id),
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'created',
                priority TEXT NOT NULL DEFAULT 'p2',
                story_type TEXT NOT NULL DEFAULT 'feature',
                tags TEXT NOT NULL DEFAULT '[]',
                task_count INTEGER NOT NULL DEFAULT 0,
                context TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await
        .expect("应能创建旧版 stories 表");

        let repo = SqliteStoryRepository::new(pool);
        repo.initialize().await.expect("初始化时应能自动补齐缺失列");
        repo
    }

    #[tokio::test]
    async fn initialize_adds_default_workspace_id_for_legacy_story_table() {
        let repo = new_repo_with_legacy_story_table().await;
        let columns = sqlx::query(
            "SELECT column_name AS name
             FROM information_schema.columns
             WHERE table_schema = 'public' AND table_name = 'stories'",
        )
            .fetch_all(&repo.pool)
            .await
            .expect("应能读取 stories 表结构");

        let has_default_workspace_id = columns.iter().any(|row| {
            row.try_get::<String, _>("name")
                .map(|name| name == "default_workspace_id")
                .unwrap_or(false)
        });

        assert!(
            has_default_workspace_id,
            "initialize 后 stories 表应包含 default_workspace_id 列"
        );
    }

    #[tokio::test]
    async fn legacy_story_table_can_read_story_after_initialize() {
        let repo = new_repo_with_legacy_story_table().await;
        let project_id = uuid::Uuid::new_v4();
        let story = Story::new(project_id, "Story".to_string(), "desc".to_string());

        sqlx::query("INSERT INTO projects (id) VALUES ($1)")
            .bind(project_id.to_string())
            .execute(&repo.pool)
            .await
            .expect("应能插入 project");

        repo.create(&story)
            .await
            .expect("补齐列后应能按新 schema 写入 story");

        let loaded = repo
            .get_by_id(story.id)
            .await
            .expect("应能查询 story")
            .expect("story 应存在");

        assert_eq!(loaded.id, story.id);
        assert_eq!(loaded.default_workspace_id, None);
    }
}
